class Tables::TimeEntries < Tables::Base
  def self.table(migration)
    create_table migration do |t|
      t.references :work_package, null: false
      t.references :user, null: false
      t.float      :hours
      t.date       :spent_on
    end
  end
end
