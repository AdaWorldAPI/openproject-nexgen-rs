class WorkPackagesController < ApplicationController
  def create
    @work_package = WorkPackage.new(wp_params)
    if @work_package.save
      redirect_to projects_path
    else
      render :new
    end
  end
end
